# Reference solution for the yamaa benchmark adam-adsl-investigator-comment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL (one record per subject) from DM.

CMNT carries the investigator comment exactly as recorded; CMNTFL is
"Y" when a comment was collected and "N" when none was. An empty
comment field (quoted or not) counts as no comment; a field holding
only spaces is still a comment and its spaces are kept.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

FIELDNAMES = ["STUDYID", "USUBJID", "CMNT", "CMNTFL"]


def main():
    with open(
        os.path.join(INPUT_DIR, "dm.csv"), newline="", encoding="utf-8"
    ) as handle:
        dm_rows = list(csv.DictReader(handle))

    adsl_rows = []
    for row in dm_rows:
        comment = row.get("COMMENT") or ""
        collected = comment != ""
        adsl_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "CMNT": comment if collected else "",
                "CMNTFL": "Y" if collected else "N",
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
