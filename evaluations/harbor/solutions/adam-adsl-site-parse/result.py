# Reference solution for the yamaa benchmark adam-adsl-site-parse (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADSL from DM: parse the site code out of the subject identifier.

One record per subject. SITEIDP is the middle part of the unique subject
identifier when it has three hyphen-separated parts and the last part is
exactly four digits; otherwise it is blank. SITEID is the parsed site when
present, else the collected site, else "UNKNOWN". SUBJREF joins the site
to use and the collected subject number with a colon, and is "UNKNOWN"
when the subject number is missing.
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
        siteidp = parse_site(rec["USUBJID"])
        collected_site = rec["SITEID"]
        if siteidp:
            siteid = siteidp
        elif collected_site:
            siteid = collected_site
        else:
            siteid = "UNKNOWN"
        subjid = rec["SUBJID"]
        if subjid:
            subjref = siteid + ":" + subjid
        else:
            subjref = "UNKNOWN"
        rows.append(
            {
                "STUDYID": rec["STUDYID"],
                "USUBJID": rec["USUBJID"],
                "SUBJID": subjid,
                "SITEIDP": siteidp,
                "SITEID": siteid,
                "SUBJREF": subjref,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "SUBJID", "SITEIDP", "SITEID", "SUBJREF"]
    with open(os.path.join(OUTPUT_DIR, "adsl.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
