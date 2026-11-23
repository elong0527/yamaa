# Reference solution for the yamaa benchmark adam-advs-bsa (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADVS dataset with derived body surface area records.

Reads the collected ADVS dataset from /app/input, keeps every collected
record, and adds one body surface area (BSA) record for each weight result
(PARAMCD "WEIGHT"), at the weight's visit. Height (PARAMCD "HEIGHT") is
collected once per subject, so every weight uses the subject's height
result, whatever visit collected it. BSA uses the Mosteller formula,
sqrt(height in cm * weight in kg / 3600), and is not rounded. A weight with
no result, or a subject with no height result, gets no BSA record.
"""

import csv
import math
import os

INPUT_PATH = "/app/input/advs.csv"
OUTPUT_PATH = "/app/output/advs.csv"

COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT"]


def main():
    with open(INPUT_PATH, newline="") as f:
        collected = list(csv.DictReader(f))

    # The subject's one height result.
    heights = {}
    for row in collected:
        aval = (row.get("AVAL") or "").strip()
        if row["PARAMCD"] == "HEIGHT" and aval:
            key = (row["STUDYID"], row["USUBJID"])
            if key in heights:
                raise ValueError(f"more than one height result for {key}")
            heights[key] = float(aval)

    out_rows = [{c: row.get(c, "") for c in COLUMNS} for row in collected]
    for row in collected:
        aval = (row.get("AVAL") or "").strip()
        key = (row["STUDYID"], row["USUBJID"])
        if row["PARAMCD"] != "WEIGHT" or not aval or key not in heights:
            continue
        bsa = math.sqrt(float(aval) * heights[key] / 3600.0)
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": "BSA",
                "PARAM": "Body Surface Area (m^2)",
                "AVAL": repr(bsa),
                "VISIT": row["VISIT"],
            }
        )

    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
