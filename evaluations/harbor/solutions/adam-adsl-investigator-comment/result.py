# Reference solution for the yamaa benchmark adam-adsl-investigator-comment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL (one record per subject) from DM and CO.

Each CO record is one comment: COVAL, then COVAL1 after one space when
the comment continues there. CMNT joins a subject's comments in COSEQ
order with "; "; CMNTFL is "Y" when the subject has a comment and "N"
when none was collected.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

FIELDNAMES = ["STUDYID", "USUBJID", "CMNT", "CMNTFL"]


def read(name):
    with open(os.path.join(INPUT_DIR, name), newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    dm_rows = read("dm.csv")
    comments = {}
    for row in sorted(read("co.csv"), key=lambda r: int(r["COSEQ"])):
        text = row["COVAL"]
        if row["COVAL1"]:
            text += " " + row["COVAL1"]
        comments.setdefault(row["USUBJID"], []).append(text)

    adsl_rows = []
    for row in dm_rows:
        texts = comments.get(row["USUBJID"], [])
        adsl_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "CMNT": "; ".join(texts),
                "CMNTFL": "Y" if texts else "N",
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(
        os.path.join(OUTPUT_DIR, "adsl.csv"),
        "w",
        newline="",
        encoding="utf-8",
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELDNAMES)
        writer.writeheader()
        writer.writerows(adsl_rows)


if __name__ == "__main__":
    main()
