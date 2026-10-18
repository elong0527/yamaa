# Reference solution for the yamaa benchmark adam-clinsite-site-level (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build CLINSITE (one record per clinical site) from ADSL.

Reads /app/input/adsl.csv and writes /app/output/clinsite.csv with
columns STUDYID, SITEID, ARM, SAFPOP, ENRLPOP.

Rules:
- A subject counts as treated when SAFFL is "Y".
- A site appears exactly when at least one subject row names it.
- SITEID is the site key exactly as collected (SITENUM); leading zeros
  are kept, so "007" and "7" are different sites.
- ARM is the alphabetically first arm among the site's treated
  subjects, blank when the site has no treated subjects.
- SAFPOP is the number of treated subjects at the site (0 when none).
- ENRLPOP is the number of enrolled subjects at the site.
"""

import csv
import os
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def main():
    with open(os.path.join(INPUT_DIR, "adsl.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    sites = OrderedDict()
    for row in rows:
        site = row["SITENUM"]
        entry = sites.setdefault(
            site,
            {"studyid": row["STUDYID"], "treated_arms": [], "safpop": 0, "enrlpop": 0},
        )
        entry["enrlpop"] += 1
        if row["SAFFL"] == "Y":
            entry["safpop"] += 1
            entry["treated_arms"].append(row["ARM"])

    out_rows = []
    for site, entry in sites.items():
        arm = min(entry["treated_arms"]) if entry["treated_arms"] else ""
        out_rows.append(
            {
                "STUDYID": entry["studyid"],
                "SITEID": site,
                "ARM": arm,
                "SAFPOP": entry["safpop"],
                "ENRLPOP": entry["enrlpop"],
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "clinsite.csv"), "w", newline="") as f:
        writer = csv.DictWriter(
            f, fieldnames=["STUDYID", "SITEID", "ARM", "SAFPOP", "ENRLPOP"]
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
