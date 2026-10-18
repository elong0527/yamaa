# Reference solution for the yamaa benchmark adam-advs-bsa (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADVS dataset with derived body surface area records.

Reads the collected ADVS dataset from /app/input, keeps every collected
record, and adds one body surface area (BSA) record for each subject and
visit that has both a height result (PARAMCD "HEIGHT") and a weight
result (PARAMCD "WEIGHT") with a value. BSA uses the Mosteller formula,
sqrt(height in cm * weight in kg / 3600), and is not rounded. A BSA
record carries DTYPE "CALCULATION"; collected records have no DTYPE.
"""

import csv
import math
import os

INPUT_PATH = "/app/input/advs.csv"
OUTPUT_PATH = "/app/output/advs.csv"

COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT", "DTYPE"]


def main():
    with open(INPUT_PATH, newline="") as f:
        collected = list(csv.DictReader(f))

    # Index height and weight results by study, subject and visit.
    heights = {}
    weights = {}
    for row in collected:
        aval = (row.get("AVAL") or "").strip()
        if not aval:
            continue
        key = (row["STUDYID"], row["USUBJID"], row["VISIT"])
        if row["PARAMCD"] == "HEIGHT":
            heights[key] = float(aval)
        elif row["PARAMCD"] == "WEIGHT":
            weights[key] = float(aval)

    # Position of the last collected record of each subject and visit,
    # so a derived BSA record follows its source records.
    last_index = {}
    for i, row in enumerate(collected):
        key = (row["STUDYID"], row["USUBJID"], row["VISIT"])
        last_index[key] = i

    out_rows = []
    for i, row in enumerate(collected):
        out = {c: row.get(c, "") for c in COLUMNS}
        out["DTYPE"] = ""
        out_rows.append(out)
        key = (row["STUDYID"], row["USUBJID"], row["VISIT"])
        if last_index[key] == i and key in heights and key in weights:
            bsa = math.sqrt(heights[key] * weights[key] / 3600.0)
            out_rows.append(
                {
                    "STUDYID": key[0],
                    "USUBJID": key[1],
                    "PARAMCD": "BSA",
                    "PARAM": "Body Surface Area (m^2)",
                    "AVAL": repr(bsa),
                    "VISIT": key[2],
                    "DTYPE": "CALCULATION",
                }
            )

    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
