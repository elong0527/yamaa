# Reference solution for the yamaa benchmark adam-adsl-site-parse (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADSL from DM: carry the site and check it in the subject identifier.

One record per subject. SITEID is the collected site. SITEIDP is the
middle part of the unique subject identifier when it has three
hyphen-separated parts and the last part is exactly four digits; otherwise
it is blank. SUBJREF joins SITEID and the collected subject number with a
colon.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_site(usubjid):
    parts = usubjid.split("-")
    if len(parts) == 3 and len(parts[2]) == 4 and parts[2].isdigit():
        return parts[1]
    return ""


def main():
    with open(os.path.join(INPUT_DIR, "dm.csv"), newline="") as f:
        dm = list(csv.DictReader(f))

    rows = []
    for rec in dm:
        siteid = rec["SITEID"]
        subjid = rec["SUBJID"]
        rows.append(
            {
                "STUDYID": rec["STUDYID"],
                "USUBJID": rec["USUBJID"],
                "SUBJID": subjid,
                "SITEID": siteid,
                "SITEIDP": parse_site(rec["USUBJID"]),
                "SUBJREF": siteid + ":" + subjid,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "SUBJID", "SITEID", "SITEIDP", "SUBJREF"]
    with open(os.path.join(OUTPUT_DIR, "adsl.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
